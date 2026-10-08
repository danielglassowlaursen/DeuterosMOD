using System;
using System.Collections.Generic;
using System.Linq;
using System.Security.Principal;
using System.Text;
using System.Threading.Tasks;

namespace Deuteros.Code.Objects.Bulletins
{
	public class Bulletin
	{
		public string BulletinText { get; set; }
		public Enums.BulletinTypes BulletinTypeName { get; set; }

		public Bulletin(Enums.BulletinTypes bulletinTypeName, string bulletinText)
		{
			BulletinTypeName = bulletinTypeName;
			BulletinText = bulletinText;
		}
	}
}
